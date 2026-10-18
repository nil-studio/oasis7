#[cfg(test)]
use super::protocol::AgentChatRequest;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
pub use oasis7_proto::player_auth::{
    sign_gameplay_action_auth_proof, verify_gameplay_action_auth_proof,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::protocol::{
    AuthoritativeSessionRegisterRequest, GameplayActionRequest, HostedStrongAuthGrant,
    PlayerAuthProof, PlayerAuthScheme, PromptControlApplyRequest, PromptControlRollbackRequest,
};

#[path = "auth/hosted_authorization.rs"]
mod hosted_authorization;
#[cfg(test)]
pub(crate) use hosted_authorization::tests::context as test_hosted_authorization_context;
pub use hosted_authorization::{
    HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV, HOSTED_AUTHORIZATION_MAX_AGE_MS,
    HOSTED_IDENTITY_V2_ENABLED_ENV, HostedAuthContextV2, hosted_identity_v2_enabled,
    hosted_identity_v2_enabled_checked, issue_hosted_authorization_grant,
};
pub(crate) use hosted_authorization::{
    authorization_nonce, verify_hosted_authorization_grant, verify_hosted_release_grant,
};
pub use oasis7_proto::player_auth::{sign_collect_data_auth_proof, verify_collect_data_auth_proof};
#[path = "auth/prompt_control.rs"]
mod prompt_control;
pub use prompt_control::{
    PromptControlAuthIntent, sign_hosted_prompt_control_strong_auth_grant,
    sign_prompt_control_apply_auth_proof, sign_prompt_control_rollback_auth_proof,
    verify_hosted_prompt_control_apply_strong_auth_grant,
    verify_hosted_prompt_control_rollback_strong_auth_grant,
    verify_prompt_control_apply_auth_proof, verify_prompt_control_rollback_auth_proof,
};
mod fragment_refill_preview;
pub use fragment_refill_preview::{
    sign_fragment_refill_preview_auth_proof, verify_fragment_refill_preview_auth_proof,
};

#[path = "auth/agent_memory_correction.rs"]
mod agent_memory_correction;
#[cfg(test)]
pub(crate) use agent_memory_correction::sign_agency_control_auth_proof_for_test;
pub(crate) use agent_memory_correction::verify_agency_control_auth_proof;
pub use oasis7_proto::player_auth::{
    sign_agent_chat_auth_proof, verify_agent_chat_auth_proof,
    verify_agent_chat_auth_proof_with_authority,
};
mod refine_quote;
pub use refine_quote::{sign_refine_quote_auth_proof, verify_refine_quote_auth_proof};
mod schedule_recipe_quote;
pub use schedule_recipe_quote::{
    sign_schedule_recipe_quote_auth_proof, verify_schedule_recipe_quote_auth_proof,
};
mod product_validation_quote;
pub use product_validation_quote::{
    sign_product_validation_quote_auth_proof, verify_product_validation_quote_auth_proof,
};
mod power_survival_quote;
pub use power_survival_quote::{
    sign_power_survival_quote_auth_proof, verify_power_survival_quote_auth_proof,
};
mod power_sale_quote;
pub use power_sale_quote::{sign_power_sale_quote_auth_proof, verify_power_sale_quote_auth_proof};
mod governance_vote_quote;
pub use governance_vote_quote::{
    sign_governance_vote_quote_auth_proof, verify_governance_vote_quote_auth_proof,
};
mod war_declaration_quote;
pub use war_declaration_quote::{
    sign_war_declaration_quote_auth_proof, verify_war_declaration_quote_auth_proof,
};
mod declare_social_edge_quote;
pub use declare_social_edge_quote::{
    sign_declare_social_edge_quote_auth_proof, verify_declare_social_edge_quote_auth_proof,
};
mod adjudicate_social_fact_quote;
pub use adjudicate_social_fact_quote::{
    sign_adjudicate_social_fact_quote_auth_proof, verify_adjudicate_social_fact_quote_auth_proof,
};
mod publish_social_fact_quote;
pub use publish_social_fact_quote::{
    sign_publish_social_fact_quote_auth_proof, verify_publish_social_fact_quote_auth_proof,
};
mod revoke_social_fact_quote;
pub use revoke_social_fact_quote::{
    sign_revoke_social_fact_quote_auth_proof, verify_revoke_social_fact_quote_auth_proof,
};
mod social_contact_quote;
pub use social_contact_quote::{
    sign_social_contact_quote_auth_proof, verify_social_contact_quote_auth_proof,
};
mod transfer_material_quote;
pub use transfer_material_quote::{
    sign_transfer_material_quote_auth_proof, verify_transfer_material_quote_auth_proof,
};
mod market_quote_decision;
pub use market_quote_decision::{
    sign_market_quote_decision_auth_proof, verify_market_quote_decision_auth_proof,
};
#[path = "auth_director_capability.rs"]
mod director_capability;
pub use director_capability::{sign_director_capability_grant, verify_director_capability_grant};

const VIEWER_PLAYER_AUTH_PAYLOAD_VERSION: u8 = 1;
const HOSTED_PLAYER_ID_PREFIX: &str = "hosted-player-";
pub const HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV: &str =
    "OASIS7_HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY";
pub const HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV: &str =
    "OASIS7_HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY";
const HOSTED_REGISTRATION_GRANT_TTL_MS: u64 = 30_000;
/// Domain-separated authority scope for player-authenticated Agent Intent chat.
///
/// The value is included in the signed request envelope.  Callers that accept
/// an authoritative V2 intent should additionally compare it with this value
/// (and with their current world/reorg tuple) before consuming the nonce.
pub use oasis7_proto::player_auth::AGENT_CHAT_AUTHORITY_SCOPE;
pub use oasis7_proto::player_auth::{VIEWER_PLAYER_AUTH_SIGNATURE_V1_PREFIX, VerifiedPlayerAuth};
const VIEWER_HOSTED_STRONG_AUTH_GRANT_PAYLOAD_VERSION: u8 = 1;
pub const VIEWER_HOSTED_STRONG_AUTH_GRANT_SIGNATURE_V1_PREFIX: &str = "awhostedgrant:v1:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PromptFieldMode {
    Unchanged,
    Clear,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptFieldPatch<'a> {
    mode: PromptFieldMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PromptFieldPatchV1 {
    Unchanged,
    Clear,
    Set(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PromptControlOperationIdentityV1 {
    operation: String,
    preview: bool,
    agent_id: String,
    player_id: String,
    session_epoch: u64,
    binding_epoch: u64,
    expected_authority_epoch: String,
    expected_version: u64,
    system_prompt: PromptFieldPatchV1,
    short_term_goal: PromptFieldPatchV1,
    long_term_goal: PromptFieldPatchV1,
    rollback_target: Option<u64>,
    updated_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptControlEnhancedSigningPayload<'a> {
    operation: &'static str,
    preview: bool,
    request_id: &'a str,
    agent_id: &'a str,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
    session_epoch: u64,
    binding_epoch: u64,
    expected_authority_epoch: &'a str,
    expected_version: u64,
    system_prompt: PromptFieldPatchV1,
    short_term_goal: PromptFieldPatchV1,
    long_term_goal: PromptFieldPatchV1,
    rollback_target: Option<u64>,
    updated_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptControlApplySigningPayload<'a> {
    operation: &'static str,
    agent_id: &'a str,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
    expected_version: Option<u64>,
    updated_by: Option<&'a str>,
    system_prompt_override: PromptFieldPatch<'a>,
    short_term_goal_override: PromptFieldPatch<'a>,
    long_term_goal_override: PromptFieldPatch<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PromptControlRollbackSigningPayload<'a> {
    operation: &'static str,
    agent_id: &'a str,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
    to_version: u64,
    expected_version: Option<u64>,
    updated_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionRegisterSigningPayload<'a> {
    operation: &'static str,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_agent_id: Option<&'a str>,
    force_rebind: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostedRegistrationGrantPayload {
    player_id: String,
    public_key: String,
    device_session_id: String,
    nonce: String,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

pub fn issue_hosted_registration_grant(
    player_id: &str,
    public_key: &str,
    device_session_id: &str,
    nonce: &str,
    issued_at_unix_ms: u64,
    issuer_private_key_hex: &str,
) -> Result<String, String> {
    let payload = HostedRegistrationGrantPayload {
        player_id: normalize_required_field(player_id, "registration grant player_id")?,
        public_key: normalize_public_key_field(public_key, "registration grant public_key")?,
        device_session_id: normalize_required_field(
            device_session_id,
            "registration grant device_session_id",
        )?,
        nonce: normalize_required_field(nonce, "registration grant nonce")?,
        issued_at_unix_ms,
        expires_at_unix_ms: issued_at_unix_ms.saturating_add(HOSTED_REGISTRATION_GRANT_TTL_MS),
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| format!("encode hosted registration grant failed: {err}"))?;
    let signing_key = signing_key_from_hex(
        issuer_private_key_hex,
        "hosted registration issuer private key",
    )?;
    let signature = signing_key.sign(bytes.as_slice());
    Ok(format!(
        "v1.{}.{}",
        hex::encode(bytes),
        hex::encode(signature.to_bytes())
    ))
}

pub fn derive_hosted_registration_issuer_public_key(
    issuer_private_key_hex: &str,
) -> Result<String, String> {
    let signing_key = signing_key_from_hex(
        issuer_private_key_hex,
        "hosted registration issuer private key",
    )?;
    Ok(hex::encode(signing_key.verifying_key().to_bytes()))
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HostedPromptControlStrongAuthGrantSigningPayload<'a> {
    operation: &'static str,
    agent_id: &'a str,
    player_id: &'a str,
    player_public_key: &'a str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HostedStrongAuthGrantSigningEnvelope<T>
where
    T: Serialize,
{
    version: u8,
    payload: T,
}

pub fn sign_session_register_auth_proof(
    request: &AuthoritativeSessionRegisterRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let player_id =
        normalize_required_field(request.player_id.as_str(), "session_register player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "session_register public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "session_register signer public key")?;
    if signer_public_key != request_public_key {
        return Err("session_register public_key does not match signer public key".to_string());
    }

    let signing_key = signing_key_from_hex(
        signer_private_key_hex,
        "session_register signer private key",
    )?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "session_register signer public key",
    )?;

    let signing_payload = build_session_register_signing_payload(
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

pub fn verify_session_register_auth_proof(
    request: &AuthoritativeSessionRegisterRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_session_register_auth_proof_inner(request, proof, true)
}

pub(crate) fn verify_session_register_auth_proof_for_recovery(
    request: &AuthoritativeSessionRegisterRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_session_register_auth_proof_inner(request, proof, false)
}

fn verify_session_register_auth_proof_inner(
    request: &AuthoritativeSessionRegisterRequest,
    proof: &PlayerAuthProof,
    require_unused_hosted_grant: bool,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let request_player_id =
        normalize_required_field(request.player_id.as_str(), "session_register player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "session_register public_key",
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
    let signing_payload = build_session_register_signing_payload(
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
    // Both the public issuer and the hosted account broker issue player IDs in
    // the hosted-player namespace.  The namespace selects the grant-required
    // path; the signed grant remains the authority for every such identity.
    let hosted_registration_nonce = if hosted_identity_v2_enabled() {
        let context = verify_hosted_authorization_grant(
            request
                .registration_grant
                .as_deref()
                .ok_or("hosted authorization v2 required")?,
            &request_player_id,
            &request_public_key,
            require_unused_hosted_grant,
        )?;
        if context.capabilities != ["player_session"] {
            return Err("hosted registration authorization required".into());
        }
        Some(authorization_nonce(&context))
    } else if request_player_id.starts_with(HOSTED_PLAYER_ID_PREFIX) {
        hosted_authorization::require_legacy_registration_opt_in()?;
        Some(verify_hosted_registration_grant(
            request.registration_grant.as_deref().ok_or_else(|| {
                "hosted session registration requires a registration grant".to_string()
            })?,
            request_player_id.as_str(),
            request_public_key.as_str(),
            require_unused_hosted_grant,
        )?)
    } else {
        None
    };
    Ok(VerifiedPlayerAuth {
        player_id: proof_player_id,
        public_key: proof_public_key,
        nonce: proof.nonce,
        hosted_registration_nonce,
    })
}

fn verify_hosted_registration_grant(
    grant: &str,
    player_id: &str,
    public_key: &str,
    require_unused: bool,
) -> Result<String, String> {
    let mut parts = grant.split('.');
    if parts.next() != Some("v1") {
        return Err("registration grant version is unsupported".to_string());
    }
    let payload_hex = parts
        .next()
        .ok_or_else(|| "registration grant payload is missing".to_string())?;
    let signature_hex = parts
        .next()
        .ok_or_else(|| "registration grant signature is missing".to_string())?;
    if parts.next().is_some() {
        return Err("registration grant format is invalid".to_string());
    }
    let payload_bytes = hex::decode(payload_hex)
        .map_err(|err| format!("decode registration grant payload failed: {err}"))?;
    let payload: HostedRegistrationGrantPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|err| format!("decode registration grant failed: {err}"))?;
    if payload.player_id != player_id || payload.public_key != public_key {
        return Err("registration grant binding does not match session request".to_string());
    }
    if now_unix_ms() > payload.expires_at_unix_ms {
        return Err("registration grant expired".to_string());
    }
    let trusted_key = std::env::var(HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV).map_err(|_| {
        "trusted hosted registration issuer public key is not configured".to_string()
    })?;
    let trusted_key_bytes = decode_hex_array::<32>(
        trusted_key.as_str(),
        "hosted registration issuer public key",
    )?;
    let verifying_key = VerifyingKey::from_bytes(&trusted_key_bytes)
        .map_err(|err| format!("parse hosted registration issuer public key failed: {err}"))?;
    let signature_bytes = hex::decode(signature_hex)
        .map_err(|err| format!("decode registration grant signature failed: {err}"))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|err| format!("parse registration grant signature failed: {err}"))?;
    verifying_key
        .verify(payload_bytes.as_slice(), &signature)
        .map_err(|err| format!("verify registration grant signature failed: {err}"))?;
    if require_unused {
        ensure_registration_grant_nonce_unused(payload.nonce.as_str())?;
    }
    Ok(payload.nonce)
}

#[path = "auth_atomic_file.rs"]
mod atomic_file;

#[path = "auth_registration_replay.rs"]
mod registration_replay;
#[path = "auth_registration_replay_lock.rs"]
mod registration_replay_lock;
use registration_replay::ensure_registration_grant_nonce_unused;
pub use registration_replay::{
    HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV, preflight_hosted_registration_replay_ledger,
};
pub(crate) use registration_replay::{
    claim_registration_grant_nonce_for_recovery, consume_registration_grant_nonce,
};
pub use registration_replay_lock::ExclusiveDirectoryProcessLock;

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn build_prompt_control_apply_signing_payload(
    intent: PromptControlAuthIntent,
    request: &PromptControlApplyRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    if has_enhanced_prompt_identity_apply(request) {
        let request_id = normalize_required_field(
            request
                .request_id
                .as_deref()
                .ok_or_else(|| "prompt_control request_id is required".to_string())?,
            "prompt_control request_id",
        )?;
        if request_id.len() > 128 {
            return Err("prompt_control request_id exceeds 128 UTF-8 bytes".to_string());
        }
        let identity = normalize_prompt_control_operation_identity(
            prompt_control_intent_operation(intent),
            matches!(intent, PromptControlAuthIntent::Preview),
            request.agent_id.as_str(),
            player_id,
            request.session_epoch,
            request.binding_epoch,
            request.expected_authority_epoch.as_deref(),
            request.expected_version,
            &request.system_prompt_override,
            &request.short_term_goal_override,
            &request.long_term_goal_override,
            None,
            request.updated_by.as_deref(),
        )?;
        let payload = PromptControlEnhancedSigningPayload {
            operation: prompt_control_intent_operation(intent),
            preview: matches!(intent, PromptControlAuthIntent::Preview),
            request_id: request_id.as_str(),
            agent_id: identity.agent_id.as_str(),
            player_id: identity.player_id.as_str(),
            public_key,
            nonce,
            session_epoch: identity.session_epoch,
            binding_epoch: identity.binding_epoch,
            expected_authority_epoch: identity.expected_authority_epoch.as_str(),
            expected_version: identity.expected_version,
            system_prompt: identity.system_prompt,
            short_term_goal: identity.short_term_goal,
            long_term_goal: identity.long_term_goal,
            rollback_target: identity.rollback_target,
            updated_by: identity.updated_by.as_deref(),
        };
        return encode_signing_payload(payload);
    }
    let payload = PromptControlApplySigningPayload {
        operation: prompt_control_intent_operation(intent),
        agent_id: request.agent_id.as_str(),
        player_id,
        public_key,
        nonce,
        expected_version: request.expected_version,
        updated_by: request.updated_by.as_deref(),
        system_prompt_override: prompt_field_patch(&request.system_prompt_override),
        short_term_goal_override: prompt_field_patch(&request.short_term_goal_override),
        long_term_goal_override: prompt_field_patch(&request.long_term_goal_override),
    };
    encode_signing_payload(payload)
}

fn build_prompt_control_rollback_signing_payload(
    request: &PromptControlRollbackRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    if has_enhanced_prompt_identity_rollback(request) {
        let request_id = normalize_required_field(
            request
                .request_id
                .as_deref()
                .ok_or_else(|| "prompt_control request_id is required".to_string())?,
            "prompt_control request_id",
        )?;
        if request_id.len() > 128 {
            return Err("prompt_control request_id exceeds 128 UTF-8 bytes".to_string());
        }
        let identity = normalize_prompt_control_operation_identity(
            "rollback",
            false,
            request.agent_id.as_str(),
            player_id,
            request.session_epoch,
            request.binding_epoch,
            request.expected_authority_epoch.as_deref(),
            request.expected_version,
            &None,
            &None,
            &None,
            Some(request.to_version),
            request.updated_by.as_deref(),
        )?;
        let payload = PromptControlEnhancedSigningPayload {
            operation: "prompt_control_rollback",
            preview: false,
            request_id: request_id.as_str(),
            agent_id: identity.agent_id.as_str(),
            player_id: identity.player_id.as_str(),
            public_key,
            nonce,
            session_epoch: identity.session_epoch,
            binding_epoch: identity.binding_epoch,
            expected_authority_epoch: identity.expected_authority_epoch.as_str(),
            expected_version: identity.expected_version,
            system_prompt: identity.system_prompt,
            short_term_goal: identity.short_term_goal,
            long_term_goal: identity.long_term_goal,
            rollback_target: identity.rollback_target,
            updated_by: identity.updated_by.as_deref(),
        };
        return encode_signing_payload(payload);
    }
    let payload = PromptControlRollbackSigningPayload {
        operation: "prompt_control_rollback",
        agent_id: request.agent_id.as_str(),
        player_id,
        public_key,
        nonce,
        to_version: request.to_version,
        expected_version: request.expected_version,
        updated_by: request.updated_by.as_deref(),
    };
    encode_signing_payload(payload)
}

fn build_session_register_signing_payload(
    request: &AuthoritativeSessionRegisterRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    let payload = SessionRegisterSigningPayload {
        operation: "session_register",
        player_id,
        public_key,
        nonce,
        requested_agent_id: request
            .requested_agent_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        force_rebind: request.force_rebind,
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

fn build_hosted_prompt_control_strong_auth_grant_payload(
    operation: &'static str,
    player_id: &str,
    player_public_key: &str,
    agent_id: &str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
) -> Result<Vec<u8>, String> {
    let payload = HostedPromptControlStrongAuthGrantSigningPayload {
        operation,
        agent_id,
        player_id,
        player_public_key,
        issued_at_unix_ms,
        expires_at_unix_ms,
    };
    let envelope = HostedStrongAuthGrantSigningEnvelope {
        version: VIEWER_HOSTED_STRONG_AUTH_GRANT_PAYLOAD_VERSION,
        payload,
    };
    serde_cbor::to_vec(&envelope)
        .map_err(|err| format!("encode hosted strong-auth grant payload failed: {err}"))
}

fn prompt_control_intent_operation(intent: PromptControlAuthIntent) -> &'static str {
    match intent {
        PromptControlAuthIntent::Preview => "prompt_control_preview",
        PromptControlAuthIntent::Apply => "prompt_control_apply",
    }
}

fn prompt_field_patch(value: &Option<Option<String>>) -> PromptFieldPatch<'_> {
    match value {
        None => PromptFieldPatch {
            mode: PromptFieldMode::Unchanged,
            value: None,
        },
        Some(None) => PromptFieldPatch {
            mode: PromptFieldMode::Clear,
            value: None,
        },
        Some(Some(next)) => PromptFieldPatch {
            mode: PromptFieldMode::Set,
            value: Some(next.as_str()),
        },
    }
}

fn prompt_field_patch_v1(value: &Option<Option<String>>) -> PromptFieldPatchV1 {
    match value {
        None => PromptFieldPatchV1::Unchanged,
        Some(None) => PromptFieldPatchV1::Clear,
        Some(Some(raw)) => {
            let value = raw.trim();
            if value.is_empty() {
                PromptFieldPatchV1::Clear
            } else {
                PromptFieldPatchV1::Set(value.to_string())
            }
        }
    }
}

pub(crate) fn has_enhanced_prompt_identity_apply(request: &PromptControlApplyRequest) -> bool {
    request.request_id.is_some()
        || request.session_epoch.is_some()
        || request.binding_epoch.is_some()
        || request.expected_authority_epoch.is_some()
}

pub(crate) fn has_enhanced_prompt_identity_rollback(
    request: &PromptControlRollbackRequest,
) -> bool {
    request.request_id.is_some()
        || request.session_epoch.is_some()
        || request.binding_epoch.is_some()
        || request.expected_authority_epoch.is_some()
}

#[expect(
    clippy::too_many_arguments,
    reason = "Prompt-control identity normalization mirrors the stable signed protocol payload"
)]
pub(crate) fn normalize_prompt_control_operation_identity(
    operation: &str,
    preview: bool,
    agent_id: &str,
    player_id: &str,
    session_epoch: Option<u64>,
    binding_epoch: Option<u64>,
    expected_authority_epoch: Option<&str>,
    expected_version: Option<u64>,
    system_prompt: &Option<Option<String>>,
    short_term_goal: &Option<Option<String>>,
    long_term_goal: &Option<Option<String>>,
    rollback_target: Option<u64>,
    updated_by: Option<&str>,
) -> Result<PromptControlOperationIdentityV1, String> {
    let agent_id = normalize_required_field(agent_id, "prompt_control agent_id")?;
    let player_id = normalize_required_field(player_id, "prompt_control player_id")?;
    let expected_authority_epoch = normalize_required_field(
        expected_authority_epoch
            .ok_or_else(|| "prompt_control expected_authority_epoch is required".to_string())?,
        "prompt_control expected_authority_epoch",
    )?;
    let updated_by = updated_by
        .map(|value| normalize_required_field(value, "prompt_control updated_by"))
        .transpose()?;
    Ok(PromptControlOperationIdentityV1 {
        operation: operation.to_string(),
        preview,
        agent_id,
        player_id,
        session_epoch: session_epoch
            .ok_or_else(|| "prompt_control session_epoch is required".to_string())?,
        binding_epoch: binding_epoch
            .ok_or_else(|| "prompt_control binding_epoch is required".to_string())?,
        expected_authority_epoch,
        expected_version: expected_version
            .ok_or_else(|| "prompt_control expected_version is required".to_string())?,
        system_prompt: prompt_field_patch_v1(system_prompt),
        short_term_goal: prompt_field_patch_v1(short_term_goal),
        long_term_goal: prompt_field_patch_v1(long_term_goal),
        rollback_target,
        updated_by,
    })
}

pub(crate) fn prompt_control_operation_digest(
    identity: &PromptControlOperationIdentityV1,
) -> String {
    crate::simulator::h_v1("oasis7.viewer.prompt-control.operation.v1", identity).to_string()
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

fn normalize_prompt_control_grant_operation(raw: &str) -> Result<&'static str, String> {
    match raw.trim() {
        "prompt_control_preview" => Ok("prompt_control_preview"),
        "prompt_control_apply" => Ok("prompt_control_apply"),
        "prompt_control_rollback" => Ok("prompt_control_rollback"),
        _ => Err("unsupported hosted strong-auth action_id".to_string()),
    }
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

#[path = "auth_hosted_strong_auth.rs"]
mod hosted_strong_auth;
use hosted_strong_auth::*;
#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
