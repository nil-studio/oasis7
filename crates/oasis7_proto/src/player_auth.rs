//! Pure signed player-request contracts shared by clients and Runtime admission.
use crate::viewer::{AgentChatRequest, GameplayActionRequest, PlayerAuthProof, PlayerAuthScheme};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::Serialize;
const VIEWER_PLAYER_AUTH_PAYLOAD_VERSION: u8 = 1;
pub const VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX: &str = "awviewauth:v1:";
pub const AGENT_CHAT_AUTHORITY_SCOPE: &str = "player_agent_chat";
mod agent_chat;
mod collect_data;
pub use agent_chat::{
    sign_agent_chat_auth_proof, verify_agent_chat_auth_proof,
    verify_agent_chat_auth_proof_with_authority,
};
pub use collect_data::{sign_collect_data_auth_proof, verify_collect_data_auth_proof};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPlayerAuth {
    pub player_id: String,
    pub public_key: String,
    pub nonce: u64,
    pub hosted_registration_nonce: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AgentChatSigningPayload<'a> {
    operation: &'static str,
    agent_id: &'a str,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_tick: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    world_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reorg_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authority_scope: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replaces_intent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_authority: Option<&'a crate::viewer::CanonicalAgentChatAuthorityV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GameplayActionSigningPayload<'a> {
    operation: &'static str,
    action_id: &'a str,
    target_agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_agent_id: Option<&'a str>,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ViewerPlayerAuthSigningEnvelope<'a, T>
where
    T: Serialize,
{
    version: u8,
    payload: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<&'a str>,
}

pub fn sign_gameplay_action_auth_proof(
    request: &GameplayActionRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let player_id =
        normalize_required_field(request.player_id.as_str(), "gameplay_action player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "gameplay_action public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "gameplay_action signer public key")?;
    if signer_public_key != request_public_key {
        return Err("gameplay_action public_key does not match signer public key".to_string());
    }

    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "gameplay_action signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "gameplay_action signer public key",
    )?;

    let signing_payload = build_gameplay_action_signing_payload(
        request,
        player_id.as_str(),
        request_public_key.as_str(),
        nonce,
    )?;
    sign_player_auth_proof(
        signing_key,
        player_id,
        signer_public_key,
        nonce,
        signing_payload,
    )
}

pub fn verify_gameplay_action_auth_proof(
    request: &GameplayActionRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let request_player_id =
        normalize_required_field(request.player_id.as_str(), "gameplay_action player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "gameplay_action public_key",
    )?;
    let proof_player_id =
        normalize_required_field(proof.player_id.as_str(), "auth proof player_id")?;
    let proof_public_key =
        normalize_public_key_field(proof.public_key.as_str(), "auth proof public key")?;
    if request_player_id != proof_player_id {
        return Err("auth proof player_id does not match request player_id".to_string());
    }
    if request_public_key != proof_public_key {
        return Err("auth proof public_key does not match request public_key".to_string());
    }
    if proof.nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let signing_payload = build_gameplay_action_signing_payload(
        request,
        proof_player_id.as_str(),
        proof_public_key.as_str(),
        proof.nonce,
    )?;
    verify_player_auth_signature(
        proof_public_key.as_str(),
        proof.signature.as_str(),
        signing_payload.as_slice(),
    )?;
    Ok(VerifiedPlayerAuth {
        player_id: proof_player_id,
        public_key: proof_public_key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    })
}

fn build_agent_chat_signing_payload(
    request: &AgentChatRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    let intent_seq = match request.intent_seq {
        Some(0) => {
            return Err("agent_chat intent_seq must be greater than zero".to_string());
        }
        Some(value) => Some(value),
        None => None,
    };
    let authority = normalize_agent_chat_authority(request)?;
    let replaces_intent_id = normalize_optional_agent_chat_field(
        request.replaces_intent_id.as_deref(),
        "agent_chat replaces_intent_id",
    )?;
    let payload = AgentChatSigningPayload {
        operation: "agent_chat",
        agent_id: request.agent_id.as_str(),
        player_id,
        public_key,
        nonce,
        message: request.message.as_str(),
        intent_tick: request.intent_tick,
        intent_seq,
        world_id: authority.as_ref().map(|(world_id, _, _)| world_id.as_str()),
        reorg_epoch: authority.as_ref().map(|(_, reorg_epoch, _)| *reorg_epoch),
        authority_scope: authority
            .as_ref()
            .map(|(_, _, authority_scope)| authority_scope.as_str()),
        replaces_intent_id: replaces_intent_id.as_deref(),
        canonical_authority: request.canonical_authority.as_ref(),
    };
    encode_signing_payload(payload)
}

fn normalize_agent_chat_authority(
    request: &AgentChatRequest,
) -> Result<Option<(String, u64, String)>, String> {
    match (
        request.world_id.as_deref(),
        request.reorg_epoch,
        request.authority_scope.as_deref(),
    ) {
        (None, None, None) => Ok(None),
        (Some(world_id), Some(reorg_epoch), Some(authority_scope)) => Ok(Some((
            normalize_required_field(world_id, "agent_chat world_id")?,
            reorg_epoch,
            normalize_required_field(authority_scope, "agent_chat authority_scope")?,
        ))),
        _ => Err(
            "agent_chat authority envelope must include world_id, reorg_epoch, and authority_scope"
                .to_string(),
        ),
    }
}

fn normalize_optional_agent_chat_field(
    raw: Option<&str>,
    label: &str,
) -> Result<Option<String>, String> {
    raw.map(|value| normalize_required_field(value, label))
        .transpose()
}

fn build_gameplay_action_signing_payload(
    request: &GameplayActionRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    let action_id =
        normalize_required_field(request.action_id.as_str(), "gameplay_action action_id")?;
    let target_agent_id = normalize_required_field(
        request.target_agent_id.as_str(),
        "gameplay_action target_agent_id",
    )?;
    let actor_agent_id = request
        .actor_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let payload = GameplayActionSigningPayload {
        operation: "gameplay_action",
        action_id: action_id.as_str(),
        target_agent_id: target_agent_id.as_str(),
        actor_agent_id,
        player_id,
        public_key,
        nonce,
    };
    encode_signing_payload(payload)
}

fn encode_signing_payload<T>(payload: T) -> Result<Vec<u8>, String>
where
    T: Serialize,
{
    let envelope = ViewerPlayerAuthSigningEnvelope {
        version: VIEWER_PLAYER_AUTH_PAYLOAD_VERSION,
        payload,
        actor: None,
    };
    serde_cbor::to_vec(&envelope).map_err(|err| format!("encode auth payload failed: {err}"))
}

fn sign_player_auth_proof(
    signing_key: SigningKey,
    player_id: String,
    public_key: String,
    nonce: u64,
    signing_payload: Vec<u8>,
) -> Result<PlayerAuthProof, String> {
    let signature: Signature = signing_key.sign(signing_payload.as_slice());
    Ok(PlayerAuthProof {
        scheme: PlayerAuthScheme::Ed25519,
        player_id,
        public_key,
        nonce,
        signature: format!(
            "{VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX}{}",
            hex::encode(signature.to_bytes())
        ),
    })
}

fn verify_player_auth_signature(
    public_key_hex: &str,
    signature: &str,
    signing_payload: &[u8],
) -> Result<(), String> {
    let public_key_bytes = decode_hex_array::<32>(public_key_hex, "auth public key")?;
    let signature_hex = signature
        .strip_prefix(VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX)
        .ok_or_else(|| "auth signature is not awviewauth:v1".to_string())?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "auth signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse auth public key failed: {err}"))?;
    verifying_key
        .verify(signing_payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|err| format!("verify auth signature failed: {err}"))
}

fn verify_proof_scheme(proof: &PlayerAuthProof) -> Result<(), String> {
    match proof.scheme {
        PlayerAuthScheme::Ed25519 => Ok(()),
    }
}

fn normalize_required_optional_field(raw: Option<&str>, label: &str) -> Result<String, String> {
    let Some(raw) = raw else {
        return Err(format!("{label} is required"));
    };
    normalize_required_field(raw, label)
}

fn normalize_required_optional_public_key(
    raw: Option<&str>,
    label: &str,
) -> Result<String, String> {
    let Some(raw) = raw else {
        return Err(format!("{label} is required"));
    };
    normalize_public_key_field(raw, label)
}

fn normalize_required_field(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(value.to_string())
}

fn normalize_public_key_field(raw: &str, label: &str) -> Result<String, String> {
    let normalized = normalize_required_field(raw, label)?;
    let bytes = decode_hex_array::<32>(normalized.as_str(), label)?;
    Ok(hex::encode(bytes))
}

fn signing_key_from_hex(private_key_hex: &str, label: &str) -> Result<SigningKey, String> {
    let private_key_bytes = decode_hex_array::<32>(private_key_hex, label)?;
    Ok(SigningKey::from_bytes(&private_key_bytes))
}

fn verify_keypair_match(
    signing_key: &SigningKey,
    public_key_hex: &str,
    label: &str,
) -> Result<(), String> {
    let expected_public_key = hex::encode(signing_key.verifying_key().to_bytes());
    if expected_public_key != public_key_hex {
        return Err(format!(
            "{label} does not match private key: expected={expected_public_key} actual={public_key_hex}"
        ));
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|err| format!("decode {label} failed: {err}"))?;
    if bytes.len() != N {
        return Err(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        ));
    }
    let mut fixed = [0_u8; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}

mod agency_control;
pub use agency_control::{sign_agency_control_auth_proof, verify_agency_control_auth_proof};
