use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptControlAuthIntent {
    Preview,
    Apply,
}

pub fn sign_prompt_control_apply_auth_proof(
    intent: PromptControlAuthIntent,
    request: &PromptControlApplyRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let player_id =
        normalize_required_field(request.player_id.as_str(), "prompt_control player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "prompt_control public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "prompt_control signer public key")?;
    if signer_public_key != request_public_key {
        return Err("prompt_control public_key does not match signer public key".to_string());
    }

    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "prompt_control signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "prompt_control signer public key",
    )?;

    let signing_payload = super::build_prompt_control_apply_signing_payload(
        intent,
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

pub fn verify_prompt_control_apply_auth_proof(
    intent: PromptControlAuthIntent,
    request: &PromptControlApplyRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let request_player_id =
        normalize_required_field(request.player_id.as_str(), "prompt_control player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "prompt_control public_key",
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
    let signing_payload = super::build_prompt_control_apply_signing_payload(
        intent,
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

pub fn sign_prompt_control_rollback_auth_proof(
    request: &PromptControlRollbackRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let player_id =
        normalize_required_field(request.player_id.as_str(), "prompt_control player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "prompt_control public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "prompt_control signer public key")?;
    if signer_public_key != request_public_key {
        return Err("prompt_control public_key does not match signer public key".to_string());
    }

    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "prompt_control signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "prompt_control signer public key",
    )?;

    let signing_payload = super::build_prompt_control_rollback_signing_payload(
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

pub fn verify_prompt_control_rollback_auth_proof(
    request: &PromptControlRollbackRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let request_player_id =
        normalize_required_field(request.player_id.as_str(), "prompt_control player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "prompt_control public_key",
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
    let signing_payload = super::build_prompt_control_rollback_signing_payload(
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

#[expect(
    clippy::too_many_arguments,
    reason = "Hosted prompt-control grant signing preserves the stable protocol field order"
)]
pub fn sign_hosted_prompt_control_strong_auth_grant(
    action_id: &str,
    player_id: &str,
    player_public_key: &str,
    agent_id: &str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<HostedStrongAuthGrant, String> {
    if issued_at_unix_ms == 0 {
        return Err(
            "hosted strong-auth grant issued_at_unix_ms must be greater than zero".to_string(),
        );
    }
    if expires_at_unix_ms <= issued_at_unix_ms {
        return Err(
            "hosted strong-auth grant expires_at_unix_ms must be greater than issued_at_unix_ms"
                .to_string(),
        );
    }
    let operation = super::normalize_prompt_control_grant_operation(action_id)?;
    let player_id = normalize_required_field(player_id, "hosted strong-auth player_id")?;
    let player_public_key =
        normalize_public_key_field(player_public_key, "hosted strong-auth player_public_key")?;
    let agent_id = normalize_required_field(agent_id, "hosted strong-auth agent_id")?;
    let signer_public_key = normalize_public_key_field(
        signer_public_key_hex,
        "hosted strong-auth signer public key",
    )?;
    let signing_key = signing_key_from_hex(
        signer_private_key_hex,
        "hosted strong-auth signer private key",
    )?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "hosted strong-auth signer public key",
    )?;
    let signing_payload = super::build_hosted_prompt_control_strong_auth_grant_payload(
        operation,
        player_id.as_str(),
        player_public_key.as_str(),
        agent_id.as_str(),
        issued_at_unix_ms,
        expires_at_unix_ms,
    )?;
    let signature = signing_key.sign(signing_payload.as_slice());
    Ok(HostedStrongAuthGrant {
        version: super::VIEWER_HOSTED_STRONG_AUTH_GRANT_PAYLOAD_VERSION,
        action_id: operation.to_string(),
        player_id,
        player_public_key,
        agent_id,
        issued_at_unix_ms,
        expires_at_unix_ms,
        signer_public_key,
        signature: format!(
            "{}{}",
            super::VIEWER_HOSTED_STRONG_AUTH_GRANT_SIGNATURE_V1_PREFIX,
            hex::encode(signature.to_bytes())
        ),
    })
}

pub fn verify_hosted_prompt_control_apply_strong_auth_grant(
    intent: PromptControlAuthIntent,
    request: &PromptControlApplyRequest,
    grant: &HostedStrongAuthGrant,
    required_signer_public_key: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    super::verify_hosted_prompt_control_strong_auth_grant(
        super::prompt_control_intent_operation(intent),
        request.agent_id.as_str(),
        request.player_id.as_str(),
        request.public_key.as_deref(),
        grant,
        required_signer_public_key,
        now_unix_ms,
    )
}

pub fn verify_hosted_prompt_control_rollback_strong_auth_grant(
    request: &PromptControlRollbackRequest,
    grant: &HostedStrongAuthGrant,
    required_signer_public_key: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    super::verify_hosted_prompt_control_strong_auth_grant(
        "prompt_control_rollback",
        request.agent_id.as_str(),
        request.player_id.as_str(),
        request.public_key.as_deref(),
        grant,
        required_signer_public_key,
        now_unix_ms,
    )
}
