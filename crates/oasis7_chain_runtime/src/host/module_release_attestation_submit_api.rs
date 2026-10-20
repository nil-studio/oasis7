use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_node::NodeRuntime;
use oasis7_runtime::Action;
use serde::{Deserialize, Serialize};

const MODULE_RELEASE_ATTESTATION_SUBMIT_PATH: &str = "/v1/chain/module-release/attestation/submit";
const MODULE_RELEASE_ATTESTATION_ERROR_INVALID_REQUEST: &str = "invalid_request";
const MODULE_RELEASE_ATTESTATION_ERROR_INTERNAL: &str = "internal_error";
const MODULE_RELEASE_ATTESTATION_ERROR_SUBMIT_FAILED: &str = "submit_failed";
const ATTESTATION_LABEL_MAX_LEN: usize = 128;
const ATTESTATION_PROOF_CID_MAX_LEN: usize = 256;

static NEXT_MODULE_RELEASE_ATTESTATION_ACTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChainModuleReleaseAttestationSubmitRequest {
    pub(super) operator_agent_id: String,
    pub(super) request_id: u64,
    pub(super) signer_node_id: String,
    pub(super) platform: String,
    pub(super) build_manifest_hash: String,
    pub(super) source_hash: String,
    pub(super) wasm_hash: String,
    pub(super) proof_cid: String,
    pub(super) builder_image_digest: String,
    pub(super) container_platform: String,
    pub(super) canonicalizer_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChainModuleReleaseAttestationSubmitResponse {
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

impl ChainModuleReleaseAttestationSubmitResponse {
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

pub(super) fn maybe_handle_module_release_attestation_submit_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    path: &str,
) -> Result<bool, String> {
    if path != MODULE_RELEASE_ATTESTATION_SUBMIT_PATH {
        return Ok(false);
    }
    if !method.eq_ignore_ascii_case("POST") {
        write_module_release_attestation_submit_error(
            stream,
            405,
            MODULE_RELEASE_ATTESTATION_ERROR_INVALID_REQUEST,
            format!("method {method} is not allowed for {MODULE_RELEASE_ATTESTATION_SUBMIT_PATH}")
                .as_str(),
        )?;
        return Ok(true);
    }
    handle_module_release_attestation_submit(stream, request_bytes, runtime)?;
    Ok(true)
}

fn handle_module_release_attestation_submit(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
) -> Result<(), String> {
    let body = match super::feedback_submit_api::extract_http_json_body(request_bytes) {
        Ok(body) => body,
        Err(err) => {
            write_module_release_attestation_submit_error(
                stream,
                400,
                MODULE_RELEASE_ATTESTATION_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let submit_request = match parse_module_release_attestation_submit_request(body) {
        Ok(request) => request,
        Err(err) => {
            write_module_release_attestation_submit_error(
                stream,
                400,
                MODULE_RELEASE_ATTESTATION_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };

    let action_id = match next_module_release_attestation_action_id() {
        Ok(action_id) => action_id,
        Err(err) => {
            write_module_release_attestation_submit_error(
                stream,
                502,
                MODULE_RELEASE_ATTESTATION_ERROR_INTERNAL,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let payload = match build_module_release_attestation_submit_action_payload(&submit_request) {
        Ok(payload) => payload,
        Err(err) => {
            write_module_release_attestation_submit_error(
                stream,
                502,
                MODULE_RELEASE_ATTESTATION_ERROR_INTERNAL,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    if let Err(err) = runtime
        .lock()
        .map_err(|_| {
            "failed to lock node runtime for module release attestation submit".to_string()
        })?
        .submit_consensus_action_payload(action_id, payload)
    {
        write_module_release_attestation_submit_error(
            stream,
            502,
            MODULE_RELEASE_ATTESTATION_ERROR_SUBMIT_FAILED,
            format!("module release attestation submit failed: {err}").as_str(),
        )?;
        return Ok(());
    }

    let now_ms = super::now_unix_ms();
    let response = ChainModuleReleaseAttestationSubmitResponse::success(action_id, now_ms);
    write_module_release_attestation_submit_json_response(stream, 200, &response).map_err(
        |err| format!("failed to write module release attestation submit response: {err}"),
    )?;
    Ok(())
}

pub(super) fn parse_module_release_attestation_submit_request(
    body: &[u8],
) -> Result<ChainModuleReleaseAttestationSubmitRequest, String> {
    let request = serde_json::from_slice::<ChainModuleReleaseAttestationSubmitRequest>(body)
        .map_err(|err| format!("invalid module release attestation submit request: {err}"))?;
    Ok(ChainModuleReleaseAttestationSubmitRequest {
        operator_agent_id: normalize_attestation_label(
            request.operator_agent_id.as_str(),
            "operator_agent_id",
        )?,
        request_id: normalize_request_id(request.request_id)?,
        signer_node_id: normalize_attestation_label(
            request.signer_node_id.as_str(),
            "signer_node_id",
        )?,
        platform: normalize_platform(request.platform.as_str())?,
        build_manifest_hash: normalize_sha256_hex(
            request.build_manifest_hash.as_str(),
            "build_manifest_hash",
        )?,
        source_hash: normalize_sha256_hex(request.source_hash.as_str(), "source_hash")?,
        wasm_hash: normalize_sha256_hex(request.wasm_hash.as_str(), "wasm_hash")?,
        proof_cid: normalize_proof_cid(request.proof_cid.as_str())?,
        builder_image_digest: normalize_builder_image_digest(
            request.builder_image_digest.as_str(),
        )?,
        container_platform: normalize_attestation_label(
            request.container_platform.as_str(),
            "container_platform",
        )?,
        canonicalizer_version: normalize_attestation_label(
            request.canonicalizer_version.as_str(),
            "canonicalizer_version",
        )?,
    })
}

pub(super) fn build_module_release_attestation_submit_action_payload(
    request: &ChainModuleReleaseAttestationSubmitRequest,
) -> Result<Vec<u8>, String> {
    let action = Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: request.operator_agent_id.clone(),
        request_id: request.request_id,
        signer_node_id: request.signer_node_id.clone(),
        platform: request.platform.clone(),
        build_manifest_hash: request.build_manifest_hash.clone(),
        source_hash: request.source_hash.clone(),
        wasm_hash: request.wasm_hash.clone(),
        proof_cid: request.proof_cid.clone(),
        builder_image_digest: request.builder_image_digest.clone(),
        container_platform: request.container_platform.clone(),
        canonicalizer_version: request.canonicalizer_version.clone(),
    };
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action(action);
    encode_consensus_action_payload(&envelope)
}

fn normalize_request_id(raw: u64) -> Result<u64, String> {
    if raw == 0 {
        return Err("module release attestation request_id must be > 0".to_string());
    }
    Ok(raw)
}

fn normalize_platform(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("module release attestation platform is empty".to_string());
    }
    if normalized.len() > ATTESTATION_LABEL_MAX_LEN {
        return Err(format!(
            "module release attestation platform exceeds {ATTESTATION_LABEL_MAX_LEN} chars"
        ));
    }
    Ok(normalized)
}

fn normalize_sha256_hex(raw: &str, field: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "module release attestation {field} must be 64-char hex"
        ));
    }
    Ok(normalized)
}

fn normalize_builder_image_digest(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    let Some(digest_hex) = normalized.strip_prefix("sha256:") else {
        return Err(
            "module release attestation builder_image_digest must be sha256:<64-hex>".to_string(),
        );
    };
    if digest_hex.len() != 64 || !digest_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            "module release attestation builder_image_digest must be sha256:<64-hex>".to_string(),
        );
    }
    Ok(normalized)
}

fn normalize_attestation_label(raw: &str, field: &str) -> Result<String, String> {
    let normalized = raw.trim().to_string();
    if normalized.is_empty() {
        return Err(format!("module release attestation {field} is empty"));
    }
    if normalized.len() > ATTESTATION_LABEL_MAX_LEN {
        return Err(format!(
            "module release attestation {field} exceeds {ATTESTATION_LABEL_MAX_LEN} chars"
        ));
    }
    Ok(normalized)
}

fn normalize_proof_cid(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_string();
    if normalized.is_empty() {
        return Err("module release attestation proof_cid is empty".to_string());
    }
    if normalized.len() > ATTESTATION_PROOF_CID_MAX_LEN {
        return Err(format!(
            "module release attestation proof_cid exceeds {ATTESTATION_PROOF_CID_MAX_LEN} chars"
        ));
    }
    Ok(normalized)
}

fn next_module_release_attestation_action_id() -> Result<u64, String> {
    let action_id = NEXT_MODULE_RELEASE_ATTESTATION_ACTION_ID.fetch_add(1, Ordering::Relaxed);
    if action_id == 0 {
        return Err("module release attestation action id allocator exhausted".to_string());
    }
    Ok(action_id)
}

fn write_module_release_attestation_submit_error(
    stream: &mut TcpStream,
    status_code: u16,
    error_code: &str,
    error: &str,
) -> Result<(), String> {
    let payload = ChainModuleReleaseAttestationSubmitResponse::error(error_code, error);
    write_module_release_attestation_submit_json_response(stream, status_code, &payload).map_err(
        |err| format!("failed to write module release attestation submit error response: {err}"),
    )
}

fn write_module_release_attestation_submit_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &ChainModuleReleaseAttestationSubmitResponse,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload).map_err(|err| {
        format!("failed to encode module release attestation submit payload: {err}")
    })?;
    super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write json response: {err}"))
}

#[cfg(test)]
#[path = "module_release_attestation_submit_api_tests.rs"]
mod tests;
